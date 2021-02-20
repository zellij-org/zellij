//! Things related to [`Screen`]s.

use std::collections::BTreeMap;
use std::os::unix::io::RawFd;
use std::sync::mpsc::Receiver;

use super::{AppInstruction, SenderWithContext};
use crate::os_input_output::OsApi;
use crate::panes::PositionAndSize;
use crate::pty_bus::{PtyInstruction, VteEvent};
use crate::tab::Tab;
use crate::{errors::ErrorContext, wasm_vm::PluginInstruction};
use crate::{layout::Layout, panes::PaneId};

/*
 * Screen
 *
 * this holds multiple tabs, each one holding multiple panes
 * it tracks the active tab and controls tab switching, all the rest
 * is performed in Tab
 *
 */
#[derive(Debug, Clone)]
pub enum ScreenInstruction {
    Pty(RawFd, VteEvent),
    Render,
    NewPane(PaneId),
    HorizontalSplit(PaneId),
    VerticalSplit(PaneId),
    WriteCharacter(Vec<u8>),
    TerminalResize,
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    MoveFocus,
    MoveFocusLeft,
    MoveFocusDown,
    MoveFocusUp,
    MoveFocusRight,
    Quit,
    ScrollUp,
    ScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveTerminalFullscreen,
    SetSelectable(PaneId, bool),
    SetMaxHeight(PaneId, usize),
    SetInvisibleBorders(PaneId, bool),
    ClosePane(PaneId),
    ApplyLayout((Layout, Vec<RawFd>)),
    NewTab(RawFd),
    SwitchTabNext,
    SwitchTabPrev,
    CloseTab,
}

/// A [`Screen`] holds multiple [`Tab`]s, each one holding multiple [`panes`](crate::client::panes).
/// It only directly controls which tab is active, delegating the rest to the individual `Tab`.
pub struct Screen {
    /// A [`ScreenInstruction`] and [`ErrorContext`] receiver.
    pub receiver: Receiver<(ScreenInstruction, ErrorContext)>,
    /// An optional maximal amount of panes allowed per [`Tab`] in this [`Screen`] instance.
    max_panes: Option<usize>,
    /// A map between this [`Screen`]'s tabs and their ID/key.
    tabs: BTreeMap<usize, Tab>,
    /// A [`PtyInstruction`] and [`ErrorContext`] sender.
    pub send_pty_instructions: SenderWithContext<PtyInstruction>,
    /// A [`PluginInstruction`] and [`ErrorContext`] sender.
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    /// An [`AppInstruction`] and [`ErrorContext`] sender.
    pub send_app_instructions: SenderWithContext<AppInstruction>,
    /// The full size of this [`Screen`].
    full_screen_ws: PositionAndSize,
    /// The index of this [`Screen`]'s active [`Tab`].
    active_tab_index: Option<usize>,
    /// The [`OsApi`] this [`Screen`] uses.
    os_api: Box<dyn OsApi>,
}

impl Screen {
    /// Creates and returns a new [`Screen`].
    pub fn new(
        receive_screen_instructions: Receiver<(ScreenInstruction, ErrorContext)>,
        send_pty_instructions: SenderWithContext<PtyInstruction>,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        send_app_instructions: SenderWithContext<AppInstruction>,
        full_screen_ws: &PositionAndSize,
        os_api: Box<dyn OsApi>,
        max_panes: Option<usize>,
    ) -> Self {
        Screen {
            receiver: receive_screen_instructions,
            max_panes,
            send_pty_instructions,
            send_plugin_instructions,
            send_app_instructions,
            full_screen_ws: *full_screen_ws,
            active_tab_index: None,
            tabs: BTreeMap::new(),
            os_api,
        }
    }

    /// Creates a new [`Tab`] in this [`Screen`], containing a single
    /// [pane](crate::client::panes) with PTY file descriptor `pane_id`.
    pub fn new_tab(&mut self, pane_id: RawFd) {
        let tab_index = self.get_new_tab_index();
        let tab = Tab::new(
            tab_index,
            &self.full_screen_ws,
            self.os_api.clone(),
            self.send_pty_instructions.clone(),
            self.send_plugin_instructions.clone(),
            self.send_app_instructions.clone(),
            self.max_panes,
            Some(PaneId::Terminal(pane_id)),
        );
        self.active_tab_index = Some(tab_index);
        self.tabs.insert(tab_index, tab);
        self.render();
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

    /// Sets this [`Screen`]'s active [`Tab`] to the next tab.
    pub fn switch_tab_next(&mut self) {
        let active_tab_id = self.get_active_tab().unwrap().index;
        let tab_ids: Vec<usize> = self.tabs.keys().copied().collect();
        let first_tab = tab_ids.get(0).unwrap();
        let active_tab_id_position = tab_ids.iter().position(|id| id == &active_tab_id).unwrap();
        if let Some(next_tab) = tab_ids.get(active_tab_id_position + 1) {
            self.active_tab_index = Some(*next_tab);
        } else {
            self.active_tab_index = Some(*first_tab);
        }
        self.render();
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the previous tab.
    pub fn switch_tab_prev(&mut self) {
        let active_tab_id = self.get_active_tab().unwrap().index;
        let tab_ids: Vec<usize> = self.tabs.keys().copied().collect();
        let first_tab = tab_ids.get(0).unwrap();
        let last_tab = tab_ids.last().unwrap();

        let active_tab_id_position = tab_ids.iter().position(|id| id == &active_tab_id).unwrap();
        if active_tab_id == *first_tab {
            self.active_tab_index = Some(*last_tab)
        } else if let Some(prev_tab) = tab_ids.get(active_tab_id_position - 1) {
            self.active_tab_index = Some(*prev_tab)
        }
        self.render();
    }

    /// Closes this [`Screen`]'s active [`Tab`], exiting the application if it happens
    /// to be the last tab.
    pub fn close_tab(&mut self) {
        let active_tab_index = self.active_tab_index.unwrap();
        if self.tabs.len() > 1 {
            self.switch_tab_prev();
        }
        let active_tab = self.tabs.remove(&active_tab_index).unwrap();
        let pane_ids = active_tab.get_pane_ids();
        // below we don't check the result of sending the CloseTab instruction to the pty thread
        // because this might be happening when the app is closing, at which point the pty thread
        // has already closed and this would result in an error
        let _ = self
            .send_pty_instructions
            .send(PtyInstruction::CloseTab(pane_ids));
        if self.tabs.is_empty() {
            self.active_tab_index = None;
            self.send_app_instructions
                .send(AppInstruction::Exit)
                .unwrap();
        }
    }

    /// Renders this [`Screen`], which amounts to rendering its active [`Tab`].
    pub fn render(&mut self) {
        if let Some(active_tab) = self.get_active_tab_mut() {
            if active_tab.get_active_pane().is_some() {
                active_tab.render();
            } else {
                self.close_tab();
            }
        };
    }

    /// Returns a mutable reference to this [`Screen`]'s tabs.
    pub fn get_tabs_mut(&mut self) -> &mut BTreeMap<usize, Tab> {
        &mut self.tabs
    }

    /// Returns an immutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab(&self) -> Option<&Tab> {
        match self.active_tab_index {
            Some(tab) => self.tabs.get(&tab),
            None => None,
        }
    }

    /// Returns a mutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab_mut(&mut self) -> Option<&mut Tab> {
        match self.active_tab_index {
            Some(tab_index) => self.get_tabs_mut().get_mut(&tab_index),
            None => None,
        }
    }

    /// Creates a new [`Tab`] in this [`Screen`], applying the specified [`Layout`]
    /// and switching to it.
    pub fn apply_layout(&mut self, layout: Layout, new_pids: Vec<RawFd>) {
        let tab_index = self.get_new_tab_index();
        let mut tab = Tab::new(
            tab_index,
            &self.full_screen_ws,
            self.os_api.clone(),
            self.send_pty_instructions.clone(),
            self.send_plugin_instructions.clone(),
            self.send_app_instructions.clone(),
            self.max_panes,
            None,
        );
        tab.apply_layout(layout, new_pids);
        self.active_tab_index = Some(tab_index);
        self.tabs.insert(tab_index, tab);
    }

    pub fn get_terminal_size(&self, fd: Option<RawFd>) -> PositionAndSize {
        let fd: RawFd = fd.unwrap_or_default();
        self.os_api.get_terminal_size_using_fd(fd)
    }
    pub fn set_terminal_size(&mut self, fd: Option<RawFd>, cols: usize, rows: usize) {
        let fd: RawFd = fd.unwrap_or_default();
        self.os_api
            .set_terminal_size_using_fd(fd, cols as u16, rows as u16);
    }
}
