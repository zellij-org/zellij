//! Things related to [`Screen`]s.

use std::collections::{BTreeMap, HashSet};
use std::os::unix::io::RawFd;
use std::str;

use zellij_utils::pane_size::Size;
use zellij_utils::{input::layout::Layout, position::Position, zellij_tile};

use crate::{
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    tab::{Output, Tab},
    thread_bus::Bus,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PluginCapabilities, TabInfo};
use zellij_utils::{
    errors::{ContextType, ScreenContext},
    input::{get_mode_info, options::Options},
    ipc::ClientAttributes,
};

/// Instructions that can be sent to the [`Screen`].
#[derive(Debug, Clone)]
pub(crate) enum ScreenInstruction {
    PtyBytes(RawFd, VteBytes),
    Render,
    NewPane(PaneId, ClientOrTabIndex),
    HorizontalSplit(PaneId, ClientId),
    VerticalSplit(PaneId, ClientId),
    WriteCharacter(Vec<u8>, ClientId),
    ResizeLeft(ClientId),
    ResizeRight(ClientId),
    ResizeDown(ClientId),
    ResizeUp(ClientId),
    ResizeIncrease(ClientId),
    ResizeDecrease(ClientId),
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
    NewTab(Layout, Vec<RawFd>, ClientId),
    SwitchTabNext(ClientId),
    SwitchTabPrev(ClientId),
    ToggleActiveSyncTab(ClientId),
    CloseTab(ClientId),
    GoToTab(u32, Option<ClientId>), // this Option is a hacky workaround, please do not copy thie behaviour
    ToggleTab(ClientId),
    UpdateTabName(Vec<u8>, ClientId),
    TerminalResize(Size),
    ChangeMode(ModeInfo, ClientId),
    LeftClick(Position, ClientId),
    RightClick(Position, ClientId),
    MouseRelease(Position, ClientId),
    MouseHold(Position, ClientId),
    Copy(ClientId),
    AddClient(ClientId),
    RemoveClient(ClientId),
}

impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::PtyBytes(..) => ScreenContext::HandlePtyBytes,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::NewPane(..) => ScreenContext::NewPane,
            ScreenInstruction::HorizontalSplit(..) => ScreenContext::HorizontalSplit,
            ScreenInstruction::VerticalSplit(..) => ScreenContext::VerticalSplit,
            ScreenInstruction::WriteCharacter(..) => ScreenContext::WriteCharacter,
            ScreenInstruction::ResizeLeft(..) => ScreenContext::ResizeLeft,
            ScreenInstruction::ResizeRight(..) => ScreenContext::ResizeRight,
            ScreenInstruction::ResizeDown(..) => ScreenContext::ResizeDown,
            ScreenInstruction::ResizeUp(..) => ScreenContext::ResizeUp,
            ScreenInstruction::ResizeIncrease(..) => ScreenContext::ResizeIncrease,
            ScreenInstruction::ResizeDecrease(..) => ScreenContext::ResizeDecrease,
            ScreenInstruction::SwitchFocus(..) => ScreenContext::SwitchFocus,
            ScreenInstruction::FocusNextPane(..) => ScreenContext::FocusNextPane,
            ScreenInstruction::FocusPreviousPane(..) => ScreenContext::FocusPreviousPane,
            ScreenInstruction::MoveFocusLeft(..) => ScreenContext::MoveFocusLeft,
            ScreenInstruction::MoveFocusLeftOrPreviousTab(..) => {
                ScreenContext::MoveFocusLeftOrPreviousTab
            }
            ScreenInstruction::MoveFocusDown(..) => ScreenContext::MoveFocusDown,
            ScreenInstruction::MoveFocusUp(..) => ScreenContext::MoveFocusUp,
            ScreenInstruction::MoveFocusRight(..) => ScreenContext::MoveFocusRight,
            ScreenInstruction::MoveFocusRightOrNextTab(..) => {
                ScreenContext::MoveFocusRightOrNextTab
            }
            ScreenInstruction::MovePane(..) => ScreenContext::MovePane,
            ScreenInstruction::MovePaneDown(..) => ScreenContext::MovePaneDown,
            ScreenInstruction::MovePaneUp(..) => ScreenContext::MovePaneUp,
            ScreenInstruction::MovePaneRight(..) => ScreenContext::MovePaneRight,
            ScreenInstruction::MovePaneLeft(..) => ScreenContext::MovePaneLeft,
            ScreenInstruction::Exit => ScreenContext::Exit,
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
            }
            ScreenInstruction::TogglePaneFrames => ScreenContext::TogglePaneFrames,
            ScreenInstruction::SetSelectable(..) => ScreenContext::SetSelectable,
            ScreenInstruction::ClosePane(..) => ScreenContext::ClosePane,
            ScreenInstruction::NewTab(..) => ScreenContext::NewTab,
            ScreenInstruction::SwitchTabNext(..) => ScreenContext::SwitchTabNext,
            ScreenInstruction::SwitchTabPrev(..) => ScreenContext::SwitchTabPrev,
            ScreenInstruction::CloseTab(..) => ScreenContext::CloseTab,
            ScreenInstruction::GoToTab(..) => ScreenContext::GoToTab,
            ScreenInstruction::UpdateTabName(..) => ScreenContext::UpdateTabName,
            ScreenInstruction::TerminalResize(..) => ScreenContext::TerminalResize,
            ScreenInstruction::ChangeMode(..) => ScreenContext::ChangeMode,
            ScreenInstruction::ToggleActiveSyncTab(..) => ScreenContext::ToggleActiveSyncTab,
            ScreenInstruction::ScrollUpAt(..) => ScreenContext::ScrollUpAt,
            ScreenInstruction::ScrollDownAt(..) => ScreenContext::ScrollDownAt,
            ScreenInstruction::LeftClick(..) => ScreenContext::LeftClick,
            ScreenInstruction::RightClick(..) => ScreenContext::RightClick,
            ScreenInstruction::MouseRelease(..) => ScreenContext::MouseRelease,
            ScreenInstruction::MouseHold(..) => ScreenContext::MouseHold,
            ScreenInstruction::Copy(..) => ScreenContext::Copy,
            ScreenInstruction::ToggleTab(..) => ScreenContext::ToggleTab,
            ScreenInstruction::AddClient(..) => ScreenContext::AddClient,
            ScreenInstruction::RemoveClient(..) => ScreenContext::RemoveClient,
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
    /// The indices of this [`Screen`]'s active [`Tab`]s.
    active_tab_indices: BTreeMap<ClientId, usize>,
    tab_history: BTreeMap<ClientId, Vec<usize>>,
    mode_info: ModeInfo,
    colors: Palette,
    draw_pane_frames: bool,
}

impl Screen {
    /// Creates and returns a new [`Screen`].
    pub fn new(
        bus: Bus<ScreenInstruction>,
        client_attributes: &ClientAttributes,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        draw_pane_frames: bool,
    ) -> Self {
        Screen {
            bus,
            max_panes,
            size: client_attributes.size,
            colors: client_attributes.palette,
            active_tab_indices: BTreeMap::new(),
            tabs: BTreeMap::new(),
            tab_history: BTreeMap::new(),
            mode_info,
            draw_pane_frames,
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

    fn move_clients_from_closed_tab(&mut self, previous_tab_index: usize) {
        let client_ids_in_closed_tab: Vec<ClientId> = self
            .active_tab_indices
            .iter()
            .filter(|(_c_id, t_index)| **t_index == previous_tab_index)
            .map(|(c_id, _t_index)| c_id)
            .copied()
            .collect();
        for client_id in client_ids_in_closed_tab {
            let client_previous_tab = self.tab_history.get_mut(&client_id).unwrap().pop().unwrap();
            self.active_tab_indices
                .insert(client_id, client_previous_tab);
            self.tabs
                .get_mut(&client_previous_tab)
                .unwrap()
                .add_client(client_id);
        }
    }
    fn move_clients(&mut self, source_index: usize, destination_index: usize) {
        let connected_clients_in_source_tab = {
            let source_tab = self.tabs.get_mut(&source_index).unwrap();
            source_tab.drain_connected_clients()
        };
        let destination_tab = self.tabs.get_mut(&destination_index).unwrap();
        destination_tab.add_multiple_clients(&connected_clients_in_source_tab);
    }
    /// A helper function to switch to a new tab at specified position.
    fn switch_active_tab(&mut self, new_tab_pos: usize, client_id: ClientId) {
        if let Some(new_tab) = self.tabs.values().find(|t| t.position == new_tab_pos) {
            let current_tab = self.get_active_tab(client_id).unwrap();

            // If new active tab is same as the current one, do nothing.
            if current_tab.position == new_tab_pos {
                return;
            }

            current_tab.visible(false);
            let current_tab_index = current_tab.index;
            let new_tab_index = new_tab.index;
            let new_tab = self.get_indexed_tab_mut(new_tab_index).unwrap();
            new_tab.set_force_render();
            new_tab.visible(true);

            // currently all clients are just mirrors, so we perform the action for every entry in
            // tab_history
            // TODO: receive a client_id and only do it for the client
            for (client_id, tab_history) in self.tab_history.iter_mut() {
                let old_active_index = self.active_tab_indices.remove(client_id).unwrap();
                self.active_tab_indices.insert(*client_id, new_tab_index);
                tab_history.retain(|&e| e != new_tab_pos);
                tab_history.push(old_active_index);
            }

            self.move_clients(current_tab_index, new_tab_index);

            self.update_tabs();
            self.render();
        }
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the next tab.
    pub fn switch_tab_next(&mut self, client_id: ClientId) {
        let active_tab_pos = self.get_active_tab(client_id).unwrap().position;
        let new_tab_pos = (active_tab_pos + 1) % self.tabs.len();

        self.switch_active_tab(new_tab_pos, client_id);
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the previous tab.
    pub fn switch_tab_prev(&mut self, client_id: ClientId) {
        let active_tab_pos = self.get_active_tab(client_id).unwrap().position;
        let new_tab_pos = if active_tab_pos == 0 {
            self.tabs.len() - 1
        } else {
            active_tab_pos - 1
        };

        self.switch_active_tab(new_tab_pos, client_id);
    }

    pub fn go_to_tab(&mut self, tab_index: usize, client_id: ClientId) {
        self.switch_active_tab(tab_index - 1, client_id);
    }

    fn close_tab_at_index(&mut self, tab_index: usize) {
        let tab_to_close = self.tabs.remove(&tab_index).unwrap();
        let pane_ids = tab_to_close.get_pane_ids();
        // below we don't check the result of sending the CloseTab instruction to the pty thread
        // because this might be happening when the app is closing, at which point the pty thread
        // has already closed and this would result in an error
        self.bus
            .senders
            .send_to_pty(PtyInstruction::CloseTab(pane_ids))
            .unwrap();
        if self.tabs.is_empty() {
            self.active_tab_indices.clear();
            self.bus
                .senders
                .send_to_server(ServerInstruction::Render(None))
                .unwrap();
        } else {
            self.move_clients_from_closed_tab(tab_index);
            let visible_tab_indices: HashSet<usize> =
                self.active_tab_indices.values().copied().collect();
            for t in self.tabs.values_mut() {
                if visible_tab_indices.contains(&t.index) {
                    t.set_force_render();
                    t.visible(true);
                }
                if t.position > tab_to_close.position {
                    t.position -= 1;
                }
            }
            self.update_tabs();
            self.render();
        }
    }

    // Closes the client_id's focused tab
    pub fn close_tab(&mut self, client_id: ClientId) {
        let active_tab_index = *self.active_tab_indices.get(&client_id).unwrap();
        self.close_tab_at_index(active_tab_index);
    }

    pub fn resize_to_screen(&mut self, new_screen_size: Size) {
        self.size = new_screen_size;
        for (_, tab) in self.tabs.iter_mut() {
            tab.resize_whole_tab(new_screen_size);
            tab.set_force_render();
        }
        self.render();
    }

    /// Renders this [`Screen`], which amounts to rendering its active [`Tab`].
    pub fn render(&mut self) {
        let mut output = Output::default();
        let mut tabs_to_close = vec![];
        for (tab_index, tab) in self.tabs.iter_mut() {
            if tab.get_active_pane().is_some() {
                tab.render(&mut output);
            } else {
                tabs_to_close.push(*tab_index);
            }
        }
        for tab_index in tabs_to_close {
            self.close_tab_at_index(tab_index);
        }
        self.bus
            .senders
            .send_to_server(ServerInstruction::Render(Some(output)))
            .unwrap();
    }

    /// Returns a mutable reference to this [`Screen`]'s tabs.
    pub fn get_tabs_mut(&mut self) -> &mut BTreeMap<usize, Tab> {
        &mut self.tabs
    }

    /// Returns an immutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab(&self, client_id: ClientId) -> Option<&Tab> {
        match self.active_tab_indices.get(&client_id) {
            Some(tab) => self.tabs.get(tab),
            None => None,
        }
    }

    /// Returns an immutable reference to this [`Screen`]'s previous active [`Tab`].
    /// Consumes the last entry in tab history.
    pub fn get_previous_tab(&mut self, client_id: ClientId) -> Option<&Tab> {
        match self.tab_history.get_mut(&client_id).unwrap().pop() {
            Some(tab) => self.tabs.get(&tab),
            None => None,
        }
    }

    /// Returns a mutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab_mut(&mut self, client_id: ClientId) -> Option<&mut Tab> {
        match self.active_tab_indices.get(&client_id) {
            Some(tab) => self.tabs.get_mut(tab),
            None => None,
        }
    }

    /// Returns a mutable reference to this [`Screen`]'s indexed [`Tab`].
    pub fn get_indexed_tab_mut(&mut self, tab_index: usize) -> Option<&mut Tab> {
        self.get_tabs_mut().get_mut(&tab_index)
    }

    /// Creates a new [`Tab`] in this [`Screen`], applying the specified [`Layout`]
    /// and switching to it.
    pub fn new_tab(&mut self, layout: Layout, new_pids: Vec<RawFd>, client_id: ClientId) {
        let tab_index = self.get_new_tab_index();
        let position = self.tabs.len();
        let mut tab = Tab::new(
            tab_index,
            position,
            String::new(),
            self.size,
            self.bus.os_input.as_ref().unwrap().clone(),
            self.bus.senders.clone(),
            self.max_panes,
            self.mode_info.clone(),
            self.colors,
            self.draw_pane_frames,
            client_id,
        );
        tab.apply_layout(layout, new_pids, tab_index);
        if let Some(active_tab) = self.get_active_tab_mut(client_id) {
            active_tab.visible(false);
            let connected_clients = active_tab.drain_connected_clients();
            tab.add_multiple_clients(&connected_clients);
        }
        for (client_id, tab_history) in self.tab_history.iter_mut() {
            let old_active_index = self.active_tab_indices.remove(client_id).unwrap();
            self.active_tab_indices.insert(*client_id, tab_index);
            tab_history.retain(|&e| e != tab_index);
            tab_history.push(old_active_index);
        }
        tab.visible(true);
        self.tabs.insert(tab_index, tab);
        if !self.active_tab_indices.contains_key(&client_id) {
            self.add_client(client_id);
        }
        self.update_tabs();

        self.render();
    }

    pub fn add_client(&mut self, client_id: ClientId) {
        let mut tab_index = 0;
        let mut tab_history = vec![];
        if let Some((_first_client, first_active_tab_index)) = self.active_tab_indices.iter().next()
        {
            tab_index = *first_active_tab_index;
        }
        if let Some((_first_client, first_tab_history)) = self.tab_history.iter().next() {
            tab_history = first_tab_history.clone();
        }
        self.active_tab_indices.insert(client_id, tab_index);
        self.tab_history.insert(client_id, tab_history);
        self.tabs.get_mut(&tab_index).unwrap().add_client(client_id);
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        if let Some(client_tab) = self.get_active_tab_mut(client_id) {
            client_tab.remove_client(client_id);
        }
        if self.active_tab_indices.contains_key(&client_id) {
            self.active_tab_indices.remove(&client_id);
        }
        if self.tab_history.contains_key(&client_id) {
            self.tab_history.remove(&client_id);
        }
    }

    pub fn update_tabs(&self) {
        let mut tab_data = vec![];
        // TODO: right now all clients are synced, so we just take the first active_tab which is
        // the same for everyone - when this is no longer the case, we need to update the TabInfo
        // to account for this (or send multiple TabInfos)
        if let Some((_first_client, first_active_tab_index)) = self.active_tab_indices.iter().next()
        {
            for tab in self.tabs.values() {
                tab_data.push(TabInfo {
                    position: tab.position,
                    name: tab.name.clone(),
                    active: *first_active_tab_index == tab.index,
                    panes_to_hide: tab.panes_to_hide.len(),
                    is_fullscreen_active: tab.is_fullscreen_active(),
                    is_sync_panes_active: tab.is_sync_panes_active(),
                });
            }
            self.bus
                .senders
                .send_to_plugin(PluginInstruction::Update(None, Event::TabUpdate(tab_data)))
                .unwrap();
        }
    }

    pub fn update_active_tab_name(&mut self, buf: Vec<u8>, client_id: ClientId) {
        let s = str::from_utf8(&buf).unwrap();
        let active_tab = self.get_active_tab_mut(client_id).unwrap();
        match s {
            "\0" => {
                active_tab.name = String::new();
            }
            "\u{007F}" | "\u{0008}" => {
                //delete and backspace keys
                active_tab.name.pop();
            }
            c => {
                active_tab.name.push_str(c);
            }
        }
        self.update_tabs();
    }
    pub fn change_mode(&mut self, mode_info: ModeInfo, client_id: ClientId) {
        if self.mode_info.mode == InputMode::Scroll
            && (mode_info.mode == InputMode::Normal || mode_info.mode == InputMode::Locked)
        {
            self.get_active_tab_mut(client_id)
                .unwrap()
                .clear_active_terminal_scroll();
        }
        self.colors = mode_info.palette;
        self.mode_info = mode_info;
        for tab in self.tabs.values_mut() {
            tab.mode_info = self.mode_info.clone();
            tab.mark_active_pane_for_rerender();
        }
    }
    pub fn move_focus_left_or_previous_tab(&mut self, client_id: ClientId) {
        if !self
            .get_active_tab_mut(client_id)
            .unwrap()
            .move_focus_left()
        {
            self.switch_tab_prev(client_id);
        }
    }
    pub fn move_focus_right_or_next_tab(&mut self, client_id: ClientId) {
        if !self
            .get_active_tab_mut(client_id)
            .unwrap()
            .move_focus_right()
        {
            self.switch_tab_next(client_id);
        }
    }
    pub fn toggle_tab(&mut self, client_id: ClientId) {
        let tab = self.get_previous_tab(client_id);
        if let Some(t) = tab {
            let position = t.position;
            self.go_to_tab(position + 1, client_id);
        };

        self.update_tabs();
        self.render();
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
) {
    let capabilities = config_options.simplified_ui;
    let draw_pane_frames = config_options.pane_frames.unwrap_or(true);

    let mut screen = Screen::new(
        bus,
        &client_attributes,
        max_panes,
        get_mode_info(
            config_options.default_mode.unwrap_or_default(),
            client_attributes.palette,
            PluginCapabilities {
                arrow_fonts: capabilities.unwrap_or_default(),
            },
        ),
        draw_pane_frames,
    );
    loop {
        let (event, mut err_ctx) = screen
            .bus
            .recv()
            .expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Screen((&event).into()));
        match event {
            ScreenInstruction::PtyBytes(pid, vte_bytes) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_terminal_pid(pid) {
                        tab.handle_pty_bytes(pid, vte_bytes);
                        break;
                    }
                }
            }
            ScreenInstruction::Render => {
                screen.render();
            }
            ScreenInstruction::NewPane(pid, client_or_tab_index) => {
                match client_or_tab_index {
                    ClientOrTabIndex::ClientId(client_id) => {
                        screen.get_active_tab_mut(client_id).unwrap().new_pane(pid);
                    }
                    ClientOrTabIndex::TabIndex(tab_index) => {
                        screen.tabs.get_mut(&tab_index).unwrap().new_pane(pid);
                    }
                };
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
                screen.update_tabs();

                screen.render();
            }
            ScreenInstruction::HorizontalSplit(pid, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .horizontal_split(pid);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
                screen.update_tabs();

                screen.render();
            }
            ScreenInstruction::VerticalSplit(pid, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .vertical_split(pid);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
                screen.update_tabs();

                screen.render();
            }
            ScreenInstruction::WriteCharacter(bytes, client_id) => {
                let active_tab = screen.get_active_tab_mut(client_id).unwrap();
                match active_tab.is_sync_panes_active() {
                    true => active_tab.write_to_terminals_on_current_tab(bytes),
                    false => active_tab.write_to_active_terminal(bytes),
                }
            }
            ScreenInstruction::ResizeLeft(client_id) => {
                screen.get_active_tab_mut(client_id).unwrap().resize_left();

                screen.render();
            }
            ScreenInstruction::ResizeRight(client_id) => {
                screen.get_active_tab_mut(client_id).unwrap().resize_right();

                screen.render();
            }
            ScreenInstruction::ResizeDown(client_id) => {
                screen.get_active_tab_mut(client_id).unwrap().resize_down();

                screen.render();
            }
            ScreenInstruction::ResizeUp(client_id) => {
                screen.get_active_tab_mut(client_id).unwrap().resize_up();

                screen.render();
            }
            ScreenInstruction::ResizeIncrease(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .resize_increase();

                screen.render();
            }
            ScreenInstruction::ResizeDecrease(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .resize_decrease();

                screen.render();
            }
            ScreenInstruction::SwitchFocus(client_id) => {
                screen.get_active_tab_mut(client_id).unwrap().move_focus();

                screen.render();
            }
            ScreenInstruction::FocusNextPane(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .focus_next_pane();

                screen.render();
            }
            ScreenInstruction::FocusPreviousPane(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .focus_previous_pane();

                screen.render();
            }
            ScreenInstruction::MoveFocusLeft(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_focus_left();

                screen.render();
            }
            ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id) => {
                screen.move_focus_left_or_previous_tab(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::MoveFocusDown(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_focus_down();

                screen.render();
            }
            ScreenInstruction::MoveFocusRight(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_focus_right();

                screen.render();
            }
            ScreenInstruction::MoveFocusRightOrNextTab(client_id) => {
                screen.move_focus_right_or_next_tab(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::MoveFocusUp(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_focus_up();

                screen.render();
            }
            ScreenInstruction::ScrollUp(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_up();

                screen.render();
            }
            ScreenInstruction::MovePane(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_active_pane();

                screen.render();
            }
            ScreenInstruction::MovePaneDown(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_active_pane_down();

                screen.render();
            }
            ScreenInstruction::MovePaneUp(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_active_pane_up();

                screen.render();
            }
            ScreenInstruction::MovePaneRight(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_active_pane_right();

                screen.render();
            }
            ScreenInstruction::MovePaneLeft(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .move_active_pane_left();

                screen.render();
            }
            ScreenInstruction::ScrollUpAt(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_terminal_up(&point, 3);

                screen.render();
            }
            ScreenInstruction::ScrollDown(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_down();

                screen.render();
            }
            ScreenInstruction::ScrollDownAt(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_terminal_down(&point, 3);

                screen.render();
            }
            ScreenInstruction::ScrollToBottom(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_to_bottom();

                screen.render();
            }
            ScreenInstruction::PageScrollUp(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_up_page();

                screen.render();
            }
            ScreenInstruction::PageScrollDown(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_down_page();

                screen.render();
            }
            ScreenInstruction::HalfPageScrollUp(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_up_half_page();

                screen.render();
            }
            ScreenInstruction::HalfPageScrollDown(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .scroll_active_terminal_down_half_page();

                screen.render();
            }
            ScreenInstruction::ClearScroll(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .clear_active_terminal_scroll();

                screen.render();
            }
            ScreenInstruction::CloseFocusedPane(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .close_focused_pane();
                screen.update_tabs(); // update_tabs eventually calls render through the plugin thread
            }
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

                screen.render();
            }
            ScreenInstruction::ClosePane(id, client_id) => {
                match client_id {
                    Some(client_id) => {
                        screen.get_active_tab_mut(client_id).unwrap().close_pane(id);
                    }
                    None => {
                        for (_index, tab) in screen.tabs.iter_mut() {
                            if tab.get_pane_ids().iter().any(|pid| *pid == id) {
                                tab.close_pane(id);
                                break;
                            }
                        }
                    }
                }
                screen.update_tabs();
            }
            ScreenInstruction::ToggleActiveTerminalFullscreen(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .toggle_active_pane_fullscreen();
                screen.update_tabs();

                screen.render();
            }
            ScreenInstruction::TogglePaneFrames => {
                screen.draw_pane_frames = !screen.draw_pane_frames;
                for (_, tab) in screen.tabs.iter_mut() {
                    tab.set_pane_frames(screen.draw_pane_frames);
                }
                screen.render();
            }
            ScreenInstruction::SwitchTabNext(client_id) => {
                screen.switch_tab_next(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::SwitchTabPrev(client_id) => {
                screen.switch_tab_prev(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::CloseTab(client_id) => {
                screen.close_tab(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::NewTab(layout, new_pane_pids, client_id) => {
                screen.new_tab(layout, new_pane_pids, client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::GoToTab(tab_index, client_id) => {
                if let Some(client_id) =
                    client_id.or_else(|| screen.active_tab_indices.keys().next().copied())
                {
                    screen.go_to_tab(tab_index as usize, client_id);
                    screen
                        .bus
                        .senders
                        .send_to_server(ServerInstruction::UnblockInputThread)
                        .unwrap();

                    screen.render();
                }
            }
            ScreenInstruction::UpdateTabName(c, client_id) => {
                screen.update_active_tab_name(c, client_id);

                screen.render();
            }
            ScreenInstruction::TerminalResize(new_size) => {
                screen.resize_to_screen(new_size);

                screen.render();
            }
            ScreenInstruction::ChangeMode(mode_info, client_id) => {
                screen.change_mode(mode_info, client_id);

                screen.render();
            }
            ScreenInstruction::ToggleActiveSyncTab(client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .toggle_sync_panes_is_active();
                screen.update_tabs();

                screen.render();
            }
            ScreenInstruction::LeftClick(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .handle_left_click(&point);

                screen.render();
            }
            ScreenInstruction::RightClick(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .handle_right_click(&point);

                screen.render();
            }
            ScreenInstruction::MouseRelease(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .handle_mouse_release(&point);

                screen.render();
            }
            ScreenInstruction::MouseHold(point, client_id) => {
                screen
                    .get_active_tab_mut(client_id)
                    .unwrap()
                    .handle_mouse_hold(&point);

                screen.render();
            }
            ScreenInstruction::Copy(client_id) => {
                screen.get_active_tab(client_id).unwrap().copy_selection();

                screen.render();
            }
            ScreenInstruction::Exit => {
                break;
            }
            ScreenInstruction::ToggleTab(client_id) => {
                screen.toggle_tab(client_id);
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();

                screen.render();
            }
            ScreenInstruction::AddClient(client_id) => {
                screen.add_client(client_id);

                screen.render();
            }
            ScreenInstruction::RemoveClient(client_id) => {
                screen.remove_client(client_id);

                screen.render();
            }
        }
    }
}

#[cfg(test)]
#[path = "./unit/screen_tests.rs"]
mod screen_tests;
