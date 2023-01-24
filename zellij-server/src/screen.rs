//! Things related to [`Screen`]s.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::str;

use zellij_utils::data::{Direction, Resize, ResizeStrategy};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::RunCommand;
use zellij_utils::input::options::Clipboard;
use zellij_utils::pane_size::{Size, SizeInPixels};
use zellij_utils::{
    input::command::TerminalAction,
    input::layout::{FloatingPanesLayout, PaneLayout, RunPluginLocation},
    position::Position,
};

use crate::panes::alacritty_functions::xparse_color;
use crate::panes::terminal_character::AnsiCode;

use crate::{
    output::Output,
    panes::sixel::SixelImageStore,
    panes::PaneId,
    plugins::PluginInstruction,
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    tab::Tab,
    thread_bus::Bus,
    ui::overlay::{Overlay, OverlayWindow, Overlayable},
    ClientId, ServerInstruction,
};
use zellij_utils::{
    data::{Event, InputMode, ModeInfo, Palette, PaletteColor, PluginCapabilities, Style, TabInfo},
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
                $closure(active_tab, $client_id)?;
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
    PluginBytes(Vec<(u32, ClientId, VteBytes)>), // u32 is plugin_id
    Render,
    NewPane(
        PaneId,
        Option<InitialTitle>,
        Option<ShouldFloat>,
        HoldForCommand,
        ClientOrTabIndex,
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
    MovePaneUp(ClientId),
    MovePaneDown(ClientId),
    MovePaneRight(ClientId),
    MovePaneLeft(ClientId),
    Exit,
    DumpScreen(String, ClientId, bool),
    EditScrollback(ClientId),
    ScrollUp(ClientId),
    ScrollUpAt(Position, ClientId),
    ScrollDown(ClientId),
    ScrollDownAt(Position, ClientId),
    ScrollToBottom(ClientId),
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
    HoldPane(PaneId, Option<i32>, RunCommand, Option<ClientId>), // Option<i32> is the exit status
    UpdatePaneName(Vec<u8>, ClientId),
    UndoRenamePane(ClientId),
    NewTab(
        Option<TerminalAction>,
        Option<PaneLayout>,
        Vec<FloatingPanesLayout>,
        Option<String>,
        ClientId,
    ),
    ApplyLayout(
        PaneLayout,
        Vec<FloatingPanesLayout>,
        Vec<(u32, HoldForCommand)>, // new pane pids
        Vec<(u32, HoldForCommand)>, // new floating pane pids
        HashMap<RunPluginLocation, Vec<u32>>,
        usize, // tab_index
        ClientId,
    ),
    SwitchTabNext(ClientId),
    SwitchTabPrev(ClientId),
    ToggleActiveSyncTab(ClientId),
    CloseTab(ClientId),
    GoToTab(u32, Option<ClientId>), // this Option is a hacky workaround, please do not copy this behaviour
    ToggleTab(ClientId),
    UpdateTabName(Vec<u8>, ClientId),
    UndoRenameTab(ClientId),
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
    AddClient(ClientId),
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
            ScreenInstruction::MovePaneDown(..) => ScreenContext::MovePaneDown,
            ScreenInstruction::MovePaneUp(..) => ScreenContext::MovePaneUp,
            ScreenInstruction::MovePaneRight(..) => ScreenContext::MovePaneRight,
            ScreenInstruction::MovePaneLeft(..) => ScreenContext::MovePaneLeft,
            ScreenInstruction::Exit => ScreenContext::Exit,
            ScreenInstruction::DumpScreen(..) => ScreenContext::DumpScreen,
            ScreenInstruction::EditScrollback(..) => ScreenContext::EditScrollback,
            ScreenInstruction::ScrollUp(..) => ScreenContext::ScrollUp,
            ScreenInstruction::ScrollDown(..) => ScreenContext::ScrollDown,
            ScreenInstruction::ScrollToBottom(..) => ScreenContext::ScrollToBottom,
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
            ScreenInstruction::UpdateTabName(..) => ScreenContext::UpdateTabName,
            ScreenInstruction::UndoRenameTab(..) => ScreenContext::UndoRenameTab,
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
    session_is_mirrored: bool,
    copy_options: CopyOptions,
}

impl Screen {
    /// Creates and returns a new [`Screen`].
    pub fn new(
        bus: Bus<ScreenInstruction>,
        client_attributes: &ClientAttributes,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        draw_pane_frames: bool,
        session_is_mirrored: bool,
        copy_options: CopyOptions,
    ) -> Self {
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
            session_is_mirrored,
            copy_options,
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
            destination_tab
                .update_input_modes()
                .with_context(err_context)?;
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
    fn switch_active_tab(&mut self, new_tab_pos: usize, client_id: ClientId) -> Result<()> {
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
                        self.move_clients_between_tabs(current_tab_index, new_tab_index, None)
                            .with_context(err_context)?;
                        let all_connected_clients: Vec<ClientId> =
                            self.connected_clients.borrow().iter().copied().collect();
                        for client_id in all_connected_clients {
                            self.update_client_tab_focus(client_id, new_tab_index);
                        }
                    } else {
                        self.move_clients_between_tabs(
                            current_tab_index,
                            new_tab_index,
                            Some(vec![client_id]),
                        )
                        .with_context(err_context)?;
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

                    self.update_tabs().with_context(err_context)?;
                    return self.render().with_context(err_context);
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the next tab.
    pub fn switch_tab_next(&mut self, client_id: ClientId) -> Result<()> {
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
                    return self.switch_active_tab(new_tab_pos, client_id);
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the previous tab.
    pub fn switch_tab_prev(&mut self, client_id: ClientId) -> Result<()> {
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

                    return self.switch_active_tab(new_tab_pos, client_id);
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    pub fn go_to_tab(&mut self, tab_index: usize, client_id: ClientId) -> Result<()> {
        self.switch_active_tab(tab_index.saturating_sub(1), client_id)
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
            self.update_tabs().with_context(err_context)?;
            self.render().with_context(err_context)
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
        self.render().with_context(err_context)
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
    pub fn render(&mut self) -> Result<()> {
        let err_context = "failed to render screen";

        let mut output = Output::new(
            self.sixel_image_store.clone(),
            self.character_cell_size.clone(),
        );
        let mut tabs_to_close = vec![];
        let size = self.size;
        let overlay = self.overlay.clone();
        for (tab_index, tab) in &mut self.tabs {
            if tab.has_selectable_tiled_panes() {
                let vte_overlay = overlay.generate_overlay(size).context(err_context)?;
                tab.render(&mut output, Some(vte_overlay))
                    .context(err_context)?;
            } else if !tab.is_pending() {
                tabs_to_close.push(*tab_index);
            }
        }
        for tab_index in tabs_to_close {
            self.close_tab_at_index(tab_index).context(err_context)?;
        }
        if output.is_dirty() {
            let serialized_output = output.serialize().context(err_context)?;
            self.bus
                .senders
                .send_to_server(ServerInstruction::Render(Some(serialized_output)))
                .context(err_context)
        } else {
            Ok(())
        }
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
    pub fn new_tab(&mut self, tab_index: usize, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to create new tab for client {client_id:?}",);

        let client_id = if self.get_active_tab(client_id).is_ok() {
            client_id
        } else if let Some(first_client_id) = self.get_first_client_id() {
            first_client_id
        } else {
            client_id
        };

        let position = self.tabs.len();
        let tab = Tab::new(
            tab_index,
            position,
            String::new(),
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
            self.connected_clients.clone(),
            self.session_is_mirrored,
            client_id,
            self.copy_options.clone(),
            self.terminal_emulator_colors.clone(),
            self.terminal_emulator_color_codes.clone(),
        );
        self.tabs.insert(tab_index, tab);
        Ok(())
    }
    pub fn apply_layout(
        &mut self,
        layout: PaneLayout,
        floating_panes_layout: Vec<FloatingPanesLayout>,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: HashMap<RunPluginLocation, Vec<u32>>,
        tab_index: usize,
        client_id: ClientId,
    ) -> Result<()> {
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

        self.update_tabs()
            .and_then(|_| self.render())
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
        self.update_tabs().with_context(err_context)
    }

    pub fn update_tabs(&self) -> Result<()> {
        let mut plugin_updates = vec![];
        for (client_id, active_tab_index) in self.active_tab_indices.iter() {
            let mut tab_data = vec![];
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
                tab_data.push(TabInfo {
                    position: tab.position,
                    name: tab.name.clone(),
                    active: *active_tab_index == tab.index,
                    panes_to_hide: tab.panes_to_hide_count(),
                    is_fullscreen_active: tab.is_fullscreen_active(),
                    is_sync_panes_active: tab.is_sync_panes_active(),
                    are_floating_panes_visible: tab.are_floating_panes_visible(),
                    other_focused_clients,
                });
            }
            plugin_updates.push((None, Some(*client_id), Event::TabUpdate(tab_data)));
        }
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::Update(plugin_updates))
            .context("failed to update tabs")?;
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
                        self.update_tabs().with_context(err_context)
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
                            self.update_tabs()
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

    pub fn change_mode(&mut self, mode_info: ModeInfo, client_id: ClientId) -> Result<()> {
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
            if let Some(os_input) = &mut self.bus.os_input {
                let _ = os_input
                    .send_to_client(client_id, ServerToClientMsg::SwitchToMode(mode_info.mode));
            }
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
                                self.switch_tab_prev(client_id)
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
                                self.switch_tab_next(client_id)
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

        self.update_tabs().context("failed to toggle tabs")?;
        self.render()
    }

    fn unblock_input(&self) -> Result<()> {
        self.bus
            .senders
            .send_to_server(ServerInstruction::UnblockInputThread)
            .context("failed to unblock input")
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
) -> Result<()> {
    let capabilities = config_options.simplified_ui;
    let draw_pane_frames = config_options.pane_frames.unwrap_or(true);
    let session_is_mirrored = config_options.mirror_session.unwrap_or(false);
    let copy_options = CopyOptions::new(
        config_options.copy_command,
        config_options.copy_clipboard.unwrap_or_default(),
        config_options.copy_on_select.unwrap_or(true),
    );

    let mut screen = Screen::new(
        bus,
        &client_attributes,
        max_panes,
        get_mode_info(
            config_options.default_mode.unwrap_or_default(),
            &client_attributes,
            PluginCapabilities {
                arrow_fonts: capabilities.unwrap_or_default(),
            },
        ),
        draw_pane_frames,
        session_is_mirrored,
        copy_options,
    );

    loop {
        let (event, mut err_ctx) = screen
            .bus
            .recv()
            .context("failed to receive event on channel")?;
        err_ctx.add_call(ContextType::Screen((&event).into()));

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
            ScreenInstruction::PluginBytes(mut plugin_bytes) => {
                for (pid, client_id, vte_bytes) in plugin_bytes.drain(..) {
                    let all_tabs = screen.get_tabs_mut();
                    for tab in all_tabs.values_mut() {
                        if tab.has_plugin(pid) {
                            tab.handle_plugin_bytes(pid, client_id, vte_bytes)
                                .context("failed to process plugin bytes")?;
                            break;
                        }
                    }
                }
                screen.render()?;
            },
            ScreenInstruction::Render => {
                screen.render()?;
            },
            ScreenInstruction::NewPane(
                pid,
                initial_pane_title,
                should_float,
                hold_for_command,
                client_or_tab_index,
            ) => {
                match client_or_tab_index {
                    ClientOrTabIndex::ClientId(client_id) => {
                        active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab,
                                                            client_id: ClientId| tab .new_pane(pid,
                                                                                               initial_pane_title,
                                                                                               should_float,
                                                                                               Some(client_id)),
                                                                                               ?);
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
                    ClientOrTabIndex::TabIndex(tab_index) => {
                        if let Some(active_tab) = screen.tabs.get_mut(&tab_index) {
                            active_tab.new_pane(pid, initial_pane_title, should_float, None)?;
                            if let Some(hold_for_command) = hold_for_command {
                                let is_first_run = true;
                                active_tab.hold_pane(pid, None, is_first_run, hold_for_command);
                            }
                        } else {
                            log::error!("Tab index not found: {:?}", tab_index);
                        }
                    },
                };
                screen.unblock_input()?;
                screen.update_tabs()?;

                screen.render()?;
            },
            ScreenInstruction::OpenInPlaceEditor(pid, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .suppress_active_pane(pid, client_id), ?);
                screen.unblock_input()?;
                screen.update_tabs()?;

                screen.render()?;
            },
            ScreenInstruction::TogglePaneEmbedOrFloating(client_id) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_pane_embed_or_floating(client_id), ?);
                screen.unblock_input()?;
                screen.update_tabs()?; // update tabs so that the ui indication will be send to the plugins

                screen.render()?;
            },
            ScreenInstruction::ToggleFloatingPanes(client_id, default_shell) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_floating_panes(client_id, default_shell), ?);
                screen.unblock_input()?;
                screen.update_tabs()?; // update tabs so that the ui indication will be send to the plugins

                screen.render()?;
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
                screen.update_tabs()?;
                screen.render()?;
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
                screen.update_tabs()?;
                screen.render()?;
            },
            ScreenInstruction::WriteCharacter(bytes, client_id) => {
                let mut should_update_tabs = false;
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| {
                        let write_result = match tab.is_sync_panes_active() {
                            true => tab.write_to_terminals_on_current_tab(bytes),
                            false => tab.write_to_active_terminal(bytes, client_id),
                        };
                        if let Ok(true) = write_result {
                            should_update_tabs = true;
                        }
                        write_result
                    },
                    ?
                );
                if should_update_tabs {
                    screen.update_tabs()?;
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
                screen.render()?;
            },
            ScreenInstruction::SwitchFocus(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::FocusNextPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::FocusPreviousPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_previous_pane(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MoveFocusLeft(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_left(client_id),
                    ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id) => {
                screen.move_focus_left_or_previous_tab(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::MoveFocusDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_down(client_id),
                    ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MoveFocusRight(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_right(client_id),
                    ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MoveFocusRightOrNextTab(client_id) => {
                screen.move_focus_right_or_next_tab(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::MoveFocusUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_up(client_id),
                    ?
                );
                screen.render()?;
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
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::EditScrollback(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.edit_scrollback(client_id),
                    ?
                );
                screen.render()?;
            },
            ScreenInstruction::ScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_up(client_id)
                );
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::MovePane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MovePaneDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_down(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MovePaneUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_up(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MovePaneRight(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_right(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MovePaneLeft(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_left(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollUpAt(point, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_up(&point, 3, client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_down(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollDownAt(point, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_down(&point, 3, client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollToBottom(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_to_bottom(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::PageScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_page(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::PageScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_page(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::HalfPageScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_half_page(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::HalfPageScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_half_page(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ClearScroll(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .clear_active_terminal_scroll(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::CloseFocusedPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.close_focused_pane(client_id), ?
                );
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
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

                screen.render()?;
            },
            ScreenInstruction::ClosePane(id, client_id) => {
                match client_id {
                    Some(client_id) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab.close_pane(id, false));
                    },
                    None => {
                        for tab in screen.tabs.values_mut() {
                            if tab.get_all_pane_ids().contains(&id) {
                                tab.close_pane(id, false);
                                break;
                            }
                        }
                    },
                }
                screen.update_tabs()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::HoldPane(id, exit_status, run_command, client_id) => {
                let is_first_run = false;
                match client_id {
                    Some(client_id) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab.hold_pane(
                            id,
                            exit_status,
                            is_first_run,
                            run_command
                        ));
                    },
                    None => {
                        for tab in screen.tabs.values_mut() {
                            if tab.get_all_pane_ids().contains(&id) {
                                tab.hold_pane(id, exit_status, is_first_run, run_command);
                                break;
                            }
                        }
                    },
                }
                screen.update_tabs()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::UpdatePaneName(c, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_active_pane_name(c, client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::UndoRenamePane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.undo_active_rename_pane(client_id), ?
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ToggleActiveTerminalFullscreen(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_active_pane_fullscreen(client_id)
                );
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::TogglePaneFrames => {
                screen.draw_pane_frames = !screen.draw_pane_frames;
                for tab in screen.tabs.values_mut() {
                    tab.set_pane_frames(screen.draw_pane_frames);
                }
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SwitchTabNext(client_id) => {
                screen.switch_tab_next(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::SwitchTabPrev(client_id) => {
                screen.switch_tab_prev(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::CloseTab(client_id) => {
                screen.close_tab(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::NewTab(
                default_shell,
                layout,
                floating_panes_layout,
                tab_name,
                client_id,
            ) => {
                let tab_index = screen.get_new_tab_index();
                screen.new_tab(tab_index, client_id)?;
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::NewTab(
                        default_shell,
                        layout,
                        floating_panes_layout,
                        tab_name,
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
                    new_pane_pids,
                    new_floating_pane_pids,
                    new_plugin_ids,
                    tab_index,
                    client_id,
                )?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::GoToTab(tab_index, client_id) => {
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
                    screen.go_to_tab(tab_index as usize, client_id)?;
                    screen.unblock_input()?;
                    screen.render()?;
                }
            },
            ScreenInstruction::UpdateTabName(c, client_id) => {
                screen.update_active_tab_name(c, client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::UndoRenameTab(client_id) => {
                screen.undo_active_rename_tab(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::TerminalResize(new_size) => {
                screen.resize_to_screen(new_size)?;
                screen.render()?;
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
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ChangeModeForAllClients(mode_info) => {
                screen.change_mode_for_all_clients(mode_info)?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ToggleActiveSyncTab(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, _client_id: ClientId| tab.toggle_sync_panes_is_active()
                );
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::LeftClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_left_click(&point, client_id), ?);
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::RightClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_right_click(&point, client_id), ?);
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MiddleClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_middle_click(&point, client_id), ?);
                screen.update_tabs()?;
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::LeftMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_left_mouse_release(&point, client_id), ?);
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::RightMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_right_mouse_release(&point, client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::MiddleMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_middle_mouse_release(&point, client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::MouseHoldLeft(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_left(&point, client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::MouseHoldRight(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_right(&point, client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::MouseHoldMiddle(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_middle(&point, client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::Copy(client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .copy_selection(client_id), ?);
                screen.render()?;
            },
            ScreenInstruction::Exit => {
                break;
            },
            ScreenInstruction::ToggleTab(client_id) => {
                screen.toggle_tab(client_id)?;
                screen.unblock_input()?;
                screen.render()?;
            },
            ScreenInstruction::AddClient(client_id) => {
                screen.add_client(client_id)?;
                screen.update_tabs()?;
                screen.render()?;
            },
            ScreenInstruction::RemoveClient(client_id) => {
                screen.remove_client(client_id)?;
                screen.render()?;
            },
            ScreenInstruction::AddOverlay(overlay, _client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.get_active_overlays_mut().push(overlay);
                screen.unblock_input()?;
            },
            ScreenInstruction::RemoveOverlay(_client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.render()?;
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
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::UpdateSearch(c, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_search_term(c, client_id), ?
                );
                screen.render()?;
            },
            ScreenInstruction::SearchDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_down(client_id)
                );
                screen.render()?;
            },
            ScreenInstruction::SearchUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_up(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleCaseSensitivity(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_search_case_sensitivity(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleWrap(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_wrap(client_id)
                );
                screen.render()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleWholeWord(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_whole_words(client_id)
                );
                screen.render()?;
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
                screen.render()?;
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
                screen.render()?;
            },
        }
    }
    Ok(())
}

#[path = "./unit/screen_tests.rs"]
#[cfg(test)]
mod screen_tests;
