use super::Screen;
use crate::background_jobs::BackgroundJob;
use crate::panes::PaneId;
use crate::plugins::{PluginInstruction, PopupRequest, PromptCaller};
use crate::route::PopupScroll;
use crate::tab::{ContextMenuRequest, PopupKind, PopupMouseOutcome, PopupPlacement};
use crate::{ClientId, ServerInstruction};
use std::collections::BTreeMap;
use std::time::Instant;
use zellij_utils::data::{
    ContextMenuContext, ContextMenuEntry, ContextMenuKind, ContextMenuTarget, Event, InputMode,
    KeybindsVec, PipePopupPlacement, PopupCorner, PopupOptions,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::actions::Action;
use zellij_utils::input::context_menu::context_menu_shortcut;
use zellij_utils::input::context_menu::ContextMenuConfig;
use zellij_utils::input::layout::RunPluginOrAlias;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::pane_size::{Size, Viewport};
use zellij_utils::plugin_api::event::ProtobufContextMenuAction;
use zellij_utils::position::Position;
use zellij_utils::prompt::PromptPlacement;

const POPUP_REVEAL_FALLBACK_MS: u64 = 1000;

pub const CONTEXT_MENU_PLUGIN_ALIAS: &str = "context-menu";
pub const PIPE_POPUP_INITIAL_COLS: usize = 50;
pub const PIPE_POPUP_INITIAL_ROWS: usize = 8;

pub fn estimated_context_menu_size(
    entries: &[ContextMenuEntry],
    keybinds: &KeybindsVec,
    base_mode: InputMode,
) -> (usize, usize) {
    let widest_item = entries
        .iter()
        .filter_map(|entry| match entry {
            ContextMenuEntry::Item { label, actions } => {
                let shortcut = context_menu_shortcut(keybinds, base_mode, actions)
                    .map(|shortcut| shortcut.chars().count() + 1)
                    .unwrap_or(0);
                Some(label.chars().count() + 4 + shortcut)
            },
            ContextMenuEntry::Separator => None,
        })
        .max()
        .unwrap_or(0);
    let width = (widest_item + 3).max(16);
    let height = entries.len() + 2;
    (width, height)
}

pub fn entries_available_to_plugins(entries: Vec<ContextMenuEntry>) -> Vec<ContextMenuEntry> {
    entries
        .into_iter()
        .filter(|entry| match entry {
            ContextMenuEntry::Separator => true,
            ContextMenuEntry::Item { actions, .. } => actions
                .iter()
                .all(|action| ProtobufContextMenuAction::try_from(action.clone()).is_ok()),
        })
        .collect()
}

impl Screen {
    pub fn update_context_menu_enabled(&mut self, context_menu_enabled: bool) {
        self.context_menu_enabled = context_menu_enabled;
        for tab in self.tabs.values_mut() {
            tab.update_context_menu_enabled(context_menu_enabled);
        }
    }
    pub fn update_context_menu_config(
        &mut self,
        client_id: ClientId,
        context_menu_config: ContextMenuConfig,
    ) {
        self.context_menu_configs
            .insert(client_id, context_menu_config);
    }
    fn context_menu_config_for_client(&self, client_id: ClientId) -> &ContextMenuConfig {
        self.context_menu_configs
            .get(&client_id)
            .unwrap_or(&self.default_context_menu_config)
    }
    pub fn handle_popup_mouse_event(&mut self, event: &MouseEvent, client_id: ClientId) -> bool {
        let outcome = match self.get_active_tab_mut(client_id) {
            Ok(tab) => tab.handle_popup_mouse_event(event, client_id),
            Err(_) => None,
        };
        match outcome {
            Some(PopupMouseOutcome::CloseRequested) => {
                self.close_top_popup_for_client(client_id);
                self.render(None).non_fatal();
                true
            },
            Some(PopupMouseOutcome::Consumed) => true,
            None => false,
        }
    }
    fn client_has_focused_popup(&self, client_id: ClientId) -> bool {
        self.tabs
            .values()
            .any(|tab| tab.has_focused_popup_for_client(client_id))
    }
    fn report_popup_state(&self, client_id: ClientId) {
        let _ = self
            .bus
            .senders
            .send_to_server(ServerInstruction::PopupStateChanged(
                client_id,
                self.client_has_focused_popup(client_id),
            ));
    }
    fn unload_popups(&self, plugin_ids: &[u32]) {
        for plugin_id in plugin_ids {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(*plugin_id));
        }
    }
    pub fn scroll_popup(&mut self, client_id: ClientId, scroll: PopupScroll) -> bool {
        let Ok(tab) = self.get_active_tab(client_id) else {
            return false;
        };
        match scroll {
            PopupScroll::Up(lines) => tab.scroll_top_popup(client_id, true, lines),
            PopupScroll::Down(lines) => tab.scroll_top_popup(client_id, false, lines),
        }
    }
    pub fn dismiss_info_popups(&mut self, client_id: ClientId) -> bool {
        let mut closed_plugin_ids = vec![];
        for tab in self.tabs.values_mut() {
            closed_plugin_ids.append(&mut tab.close_info_popups(client_id));
        }
        self.unload_popups(&closed_plugin_ids);
        !closed_plugin_ids.is_empty()
    }
    pub fn close_popups_with_missing_anchor(&mut self) -> bool {
        let mut closed = vec![];
        for tab in self.tabs.values_mut() {
            closed.append(&mut tab.close_popups_with_missing_anchor());
        }
        if closed.is_empty() {
            return false;
        }
        let mut clients = vec![];
        for (client_id, plugin_id) in closed {
            self.unload_popups(&[plugin_id]);
            if !clients.contains(&client_id) {
                clients.push(client_id);
            }
        }
        for client_id in clients {
            self.forget_context_menu_unless_open(client_id);
            self.report_popup_state(client_id);
        }
        true
    }
    fn forget_context_menu_unless_open(&mut self, client_id: ClientId) {
        let menu_still_open = self
            .open_context_menus
            .get(&client_id)
            .map(|_| {
                self.tabs.values().any(|tab| {
                    tab.popup_plugin_id(client_id)
                        .and_then(|plugin_id| tab.popup_kind_of_plugin(plugin_id))
                        == Some(PopupKind::Menu)
                })
            })
            .unwrap_or(false);
        if !menu_still_open {
            self.open_context_menus.remove(&client_id);
        }
    }
    pub fn close_top_popup_for_client(&mut self, client_id: ClientId) -> bool {
        let closed = match self.get_active_tab_mut(client_id) {
            Ok(tab) => tab.close_top_popup(client_id),
            Err(_) => None,
        };
        match closed {
            Some(plugin_id) => {
                let _ = self
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::Unload(plugin_id));
                self.forget_context_menu_unless_open(client_id);
                self.report_popup_state(client_id);
                true
            },
            None => false,
        }
    }
    pub fn record_client_input(&mut self, client_id: ClientId) {
        self.last_client_input.insert(client_id, Instant::now());
    }
    pub fn record_mouse_position(&mut self, client_id: ClientId, position: Position) {
        self.last_mouse_positions.insert(client_id, position);
    }
    pub fn popup_plugin_failed(&mut self, plugin_id: u32) -> bool {
        if self.close_popup_with_plugin_id(plugin_id) {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(plugin_id));
            true
        } else {
            false
        }
    }
    pub fn context_menu_item_actions(
        &self,
        plugin_id: u32,
        client_id: ClientId,
        index: usize,
    ) -> Vec<Action> {
        let popup_belongs_to_client = self
            .get_active_tab(client_id)
            .ok()
            .and_then(|tab| tab.popup_plugin_id(client_id))
            == Some(plugin_id);
        if !popup_belongs_to_client {
            return vec![];
        }
        let Some((context, entries)) = self.open_context_menus.get(&client_id) else {
            return vec![];
        };
        match entries.get(index) {
            Some(ContextMenuEntry::Item { actions, .. }) => actions
                .iter()
                .cloned()
                .filter_map(|action| {
                    action.into_action(context.target_pane_id(), context.target_tab_id())
                })
                .collect(),
            _ => vec![],
        }
    }
    pub fn move_popups_to_new_tab(
        &mut self,
        client_id: ClientId,
        old_tab_id: usize,
        new_tab_id: usize,
    ) {
        self.open_context_menus.remove(&client_id);
        let mut closed_plugin_ids = vec![];
        for tab in self.tabs.values_mut() {
            closed_plugin_ids.append(&mut tab.close_focused_popups(client_id));
        }
        self.unload_popups(&closed_plugin_ids);
        let info_popups = self
            .tabs
            .get_mut(&old_tab_id)
            .map(|tab| tab.take_info_popups(client_id))
            .unwrap_or_default();
        if let Some(tab) = self.tabs.get_mut(&new_tab_id) {
            tab.adopt_popups(client_id, info_popups);
        } else {
            let plugin_ids: Vec<u32> = info_popups
                .iter()
                .filter_map(|popup| match popup.pane.pid() {
                    PaneId::Plugin(plugin_id) => Some(plugin_id),
                    PaneId::Terminal(_) => None,
                })
                .collect();
            self.unload_popups(&plugin_ids);
        }
        if !closed_plugin_ids.is_empty() {
            self.report_popup_state(client_id);
        }
    }
    pub fn close_popup_for_client(&mut self, client_id: ClientId) -> bool {
        self.open_context_menus.remove(&client_id);
        let mut closed_plugin_ids = vec![];
        for tab in self.tabs.values_mut() {
            closed_plugin_ids.append(&mut tab.close_popup(client_id));
        }
        let closed_any = !closed_plugin_ids.is_empty();
        for plugin_id in closed_plugin_ids {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(plugin_id));
        }
        if closed_any {
            self.report_popup_state(client_id);
        }
        closed_any
    }
    fn close_menu_popups_for_client(&mut self, client_id: ClientId) {
        self.open_context_menus.remove(&client_id);
        let mut closed_plugin_ids = vec![];
        for tab in self.tabs.values_mut() {
            closed_plugin_ids.append(&mut tab.close_menu_popups(client_id));
        }
        let closed_any = !closed_plugin_ids.is_empty();
        for plugin_id in closed_plugin_ids {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(plugin_id));
        }
        if closed_any {
            self.report_popup_state(client_id);
        }
    }
    pub fn close_popup_with_plugin_id(&mut self, plugin_id: u32) -> bool {
        let mut closed_for_client = None;
        for tab in self.tabs.values_mut() {
            if let Some(client_id) = tab.close_popup_with_plugin_id(plugin_id) {
                closed_for_client = Some(client_id);
                break;
            }
        }
        match closed_for_client {
            Some(client_id) => {
                self.forget_context_menu_unless_open(client_id);
                self.report_popup_state(client_id);
                true
            },
            None => false,
        }
    }
    pub fn close_popups_of_closed_tab(&mut self, popups: Vec<(ClientId, u32)>) {
        let mut clients = vec![];
        for (client_id, plugin_id) in popups {
            self.open_context_menus.remove(&client_id);
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(plugin_id));
            if !clients.contains(&client_id) {
                clients.push(client_id);
            }
        }
        for client_id in clients {
            self.report_popup_state(client_id);
        }
    }
    pub fn send_key_to_popup(
        &mut self,
        client_id: ClientId,
        key_with_modifier: zellij_utils::data::KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    ) -> bool {
        match self.get_active_tab_mut(client_id) {
            Ok(tab) => tab.send_key_to_popup(
                client_id,
                key_with_modifier,
                raw_bytes,
                is_kitty_keyboard_protocol,
            ),
            Err(_) => false,
        }
    }
    pub fn open_context_menu_for_pane(&mut self, request: ContextMenuRequest, client_id: ClientId) {
        let tab_count = self.tabs.len();
        let Ok(tab) = self.get_active_tab(client_id) else {
            return;
        };
        let context = ContextMenuContext {
            kind: request.kind,
            pane_id: Some(request.pane_id.into()),
            pane_is_floating: request.is_floating,
            tab_index: Some(tab.position),
            tab_id: Some(tab.id),
            tab_count,
            line: request.position.line.0.max(0) as usize,
            column: request.position.column.0,
            client_id,
        };
        self.open_context_menu(context);
    }
    pub fn open_context_menu_for_focused_pane(&mut self, client_id: ClientId) {
        let client_id = match self.get_active_tab(client_id) {
            Ok(_) => client_id,
            Err(_) => match self.get_first_client_id() {
                Some(first_client_id) => first_client_id,
                None => return,
            },
        };
        let Ok(tab) = self.get_active_tab(client_id) else {
            return;
        };
        let Some(pane) = tab.get_active_pane(client_id) else {
            return;
        };
        let request = ContextMenuRequest {
            kind: ContextMenuKind::Pane,
            pane_id: pane.pid(),
            is_floating: tab.are_floating_panes_visible(),
            position: Position::new(pane.get_content_y() as i32, pane.get_content_x() as u16),
        };
        self.open_context_menu_for_pane(request, client_id);
    }
    pub fn open_context_menu_from_plugin(
        &mut self,
        plugin_id: u32,
        client_id: ClientId,
        target: ContextMenuTarget,
        line: usize,
        column: usize,
    ) {
        let (line, column) = self.plugin_relative_to_screen(plugin_id, client_id, line, column);
        let tab_count = self.tabs.len();
        let (kind, tab_index, tab_id) = match target {
            ContextMenuTarget::Tab(tab_index) => {
                match self.tabs.values().find(|tab| tab.position == tab_index) {
                    Some(tab) => (ContextMenuKind::Tab, Some(tab.position), Some(tab.id)),
                    None => return,
                }
            },
            ContextMenuTarget::Bar => match self.get_active_tab(client_id) {
                Ok(tab) => (ContextMenuKind::Bar, Some(tab.position), Some(tab.id)),
                Err(_) => return,
            },
        };
        let context = ContextMenuContext {
            kind,
            pane_id: None,
            pane_is_floating: false,
            tab_index,
            tab_id,
            tab_count,
            line,
            column,
            client_id,
        };
        self.open_context_menu(context);
    }
    fn plugin_relative_to_screen(
        &self,
        plugin_id: u32,
        client_id: ClientId,
        line: usize,
        column: usize,
    ) -> (usize, usize) {
        let pane_offset = self
            .get_active_tab(client_id)
            .ok()
            .and_then(|tab| tab.get_pane_with_id(PaneId::Plugin(plugin_id)))
            .or_else(|| {
                self.tabs
                    .values()
                    .find_map(|tab| tab.get_pane_with_id(PaneId::Plugin(plugin_id)))
            })
            .map(|pane| (pane.get_content_y(), pane.get_content_x()));
        match pane_offset {
            Some((y, x)) => (line + y, column + x),
            None => (line, column),
        }
    }
    fn open_context_menu(&mut self, context: ContextMenuContext) {
        let client_id = context.client_id;
        let entries = entries_available_to_plugins(
            self.context_menu_config_for_client(client_id)
                .entries_for(context.kind),
        );
        if entries.is_empty() {
            return;
        }
        let keybinds = self.keybinds_for_client(client_id);
        let mut mode_info = self
            .mode_info
            .get(&client_id)
            .unwrap_or(&self.default_mode_info)
            .clone();
        let base_mode = mode_info.base_mode.unwrap_or(self.default_mode_info.mode);
        mode_info.base_mode = Some(base_mode);
        let (width, height) = estimated_context_menu_size(&entries, &keybinds, base_mode);
        let anchor = Position::new(context.line as i32, context.column as u16);
        let run_plugin_or_alias =
            match RunPluginOrAlias::from_url(CONTEXT_MENU_PLUGIN_ALIAS, &None, None, None) {
                Ok(run_plugin_or_alias) => run_plugin_or_alias,
                Err(e) => {
                    log::error!("Failed to open context menu: {}", e);
                    return;
                },
            };
        let anchor_pane = context.pane_id.map(PaneId::from);
        self.request_popup(
            client_id,
            run_plugin_or_alias,
            PopupPlacement::At(anchor),
            PopupKind::Menu,
            anchor_pane,
            width,
            height,
            vec![
                Event::ModeUpdate(mode_info),
                Event::ContextMenu(context.clone(), entries.clone()),
            ],
        );
        self.open_context_menus
            .insert(client_id, (context, entries));
    }
    pub fn open_plugin_popup_for_plugin(
        &mut self,
        requesting_plugin_id: u32,
        client_id: ClientId,
        run_plugin_or_alias: RunPluginOrAlias,
        line: usize,
        column: usize,
        width: usize,
        height: usize,
        options: PopupOptions,
    ) {
        let (line, column) =
            self.plugin_relative_to_screen(requesting_plugin_id, client_id, line, column);
        let placement = match options.corner {
            Some(corner) => PopupPlacement::Corner(corner),
            None => PopupPlacement::At(Position::new(line as i32, column as u16)),
        };
        let kind = if options.focused {
            PopupKind::Menu
        } else {
            PopupKind::Info
        };
        self.request_popup(
            client_id,
            run_plugin_or_alias,
            placement,
            kind,
            None,
            width,
            height,
            vec![],
        );
    }
    fn request_popup(
        &mut self,
        client_id: ClientId,
        run_plugin_or_alias: RunPluginOrAlias,
        placement: PopupPlacement,
        kind: PopupKind,
        anchor_pane: Option<PaneId>,
        width: usize,
        height: usize,
        initial_events: Vec<Event>,
    ) {
        if kind == PopupKind::Menu {
            self.close_menu_popups_for_client(client_id);
        }
        let Ok(tab) = self.get_active_tab(client_id) else {
            return;
        };
        let (tab_id, tab_index) = (tab.id, tab.position);
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::LoadPopup {
                run_plugin_or_alias,
                tab_id,
                tab_index,
                client_id,
                placement,
                kind,
                anchor_pane,
                size: Size {
                    rows: height,
                    cols: width,
                },
                initial_events,
                pipe: None,
            });
    }
    fn most_recently_active(&self, candidates: impl Iterator<Item = ClientId>) -> Option<ClientId> {
        candidates.max_by_key(|client_id| {
            (
                self.last_client_input.get(client_id).copied(),
                std::cmp::Reverse(*client_id),
            )
        })
    }
    pub fn pipe_popup_owner(
        &self,
        caller_pane_id: Option<PaneId>,
    ) -> Option<(ClientId, usize, Option<PaneId>)> {
        let caller_tab_id = caller_pane_id.and_then(|pane_id| {
            self.tabs
                .iter()
                .find(|(_, tab)| tab.get_pane_with_id(pane_id).is_some())
                .map(|(tab_id, _)| *tab_id)
        });
        let connected: Vec<ClientId> = self.active_tab_ids.keys().copied().collect();
        if let (Some(pane_id), Some(tab_id)) = (caller_pane_id, caller_tab_id) {
            let focused_on_caller = connected.iter().copied().filter(|client_id| {
                self.active_tab_ids.get(client_id) == Some(&tab_id)
                    && self
                        .tabs
                        .get(&tab_id)
                        .and_then(|tab| tab.get_active_pane_id(*client_id))
                        == Some(pane_id)
            });
            if let Some(client_id) = self.most_recently_active(focused_on_caller) {
                return Some((client_id, tab_id, Some(pane_id)));
            }
            let in_caller_tab = connected
                .iter()
                .copied()
                .filter(|client_id| self.active_tab_ids.get(client_id) == Some(&tab_id));
            if let Some(client_id) = self.most_recently_active(in_caller_tab) {
                return Some((client_id, tab_id, Some(pane_id)));
            }
        }
        let client_id = self.most_recently_active(connected.iter().copied())?;
        let tab_id = *self.active_tab_ids.get(&client_id)?;
        Some((client_id, tab_id, None))
    }
    pub fn open_pipe_popup(
        &mut self,
        pipe_id: String,
        run_plugin_or_alias: RunPluginOrAlias,
        caller_pane_id: Option<PaneId>,
        placement: PipePopupPlacement,
        focused: bool,
    ) {
        let Some((client_id, tab_id, pane_in_tab)) = self.pipe_popup_owner(caller_pane_id) else {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::PipePopupFailed {
                    pipe_id,
                    error: "No user is attached to this session to answer the prompt".to_owned(),
                });
            return;
        };
        self.load_answer_popup(
            PopupRequest::Cli(pipe_id),
            run_plugin_or_alias,
            client_id,
            tab_id,
            pane_in_tab,
            caller_pane_id,
            placement,
            focused,
        );
    }
    pub fn open_prompt_popup(
        &mut self,
        caller: PromptCaller,
        owner_client_id: ClientId,
        caller_pane_id: Option<PaneId>,
        run_plugin_or_alias: RunPluginOrAlias,
        placement: Option<PromptPlacement>,
        focused: bool,
    ) {
        let tab_id = match self.active_tab_ids.get(&owner_client_id) {
            Some(tab_id) if self.tabs.contains_key(tab_id) => *tab_id,
            _ => {
                let _ = self
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::PromptPopupFailed {
                        caller,
                        error: "The user this prompt is for is not attached".to_owned(),
                    });
                return;
            },
        };
        let (pipe_placement, explicit_pane) = match placement {
            None => (PipePopupPlacement::Pane, None),
            Some(PromptPlacement::Pane(pane_id)) => {
                (PipePopupPlacement::Pane, Some(PaneId::from(pane_id)))
            },
            Some(PromptPlacement::Center) => (PipePopupPlacement::Center, None),
            Some(PromptPlacement::Mouse) => (PipePopupPlacement::Mouse, None),
            Some(PromptPlacement::Cursor) => (PipePopupPlacement::Cursor, None),
            Some(PromptPlacement::Corner(corner)) => (
                match corner {
                    PopupCorner::TopLeft => PipePopupPlacement::TopLeft,
                    PopupCorner::TopRight => PipePopupPlacement::TopRight,
                    PopupCorner::BottomLeft => PipePopupPlacement::BottomLeft,
                    PopupCorner::BottomRight => PipePopupPlacement::BottomRight,
                },
                None,
            ),
        };
        let caller_pane_id = caller_pane_id.filter(|pane_id| {
            self.tabs
                .values()
                .any(|tab| tab.get_pane_with_id(*pane_id).is_some())
        });
        let placement_pane = self.tabs.get(&tab_id).and_then(|tab| {
            explicit_pane
                .or(caller_pane_id)
                .filter(|pane_id| tab.get_pane_with_id(*pane_id).is_some())
                .or_else(|| tab.get_active_pane_id(owner_client_id))
        });
        self.load_answer_popup(
            PopupRequest::Prompt(caller),
            run_plugin_or_alias,
            owner_client_id,
            tab_id,
            placement_pane,
            caller_pane_id,
            pipe_placement,
            focused,
        );
    }
    fn load_answer_popup(
        &mut self,
        popup_request: PopupRequest,
        run_plugin_or_alias: RunPluginOrAlias,
        client_id: ClientId,
        tab_id: usize,
        pane_in_tab: Option<PaneId>,
        caller_pane_id: Option<PaneId>,
        placement: PipePopupPlacement,
        focused: bool,
    ) {
        let Some(tab) = self.tabs.get(&tab_id) else {
            return;
        };
        let bounds = tab.popup_bounds();
        let caller_pane = caller_pane_id.and_then(|pane_id| {
            self.tabs
                .values()
                .find_map(|tab| tab.get_pane_with_id(pane_id))
        });
        let caller_title = caller_pane
            .map(|pane| pane.current_title())
            .unwrap_or_default();
        let caller_tab_name = caller_pane_id.and_then(|pane_id| {
            self.tabs
                .values()
                .find(|tab| tab.get_pane_with_id(pane_id).is_some())
                .filter(|caller_tab| caller_tab.id != tab_id)
                .map(|caller_tab| caller_tab.name.clone())
        });
        let pane_area = pane_in_tab
            .and_then(|pane_id| tab.get_pane_with_id(pane_id))
            .map(|pane| {
                let geom = pane.current_geom();
                Viewport {
                    x: geom.x,
                    y: geom.y,
                    rows: geom.rows.as_usize(),
                    cols: geom.cols.as_usize(),
                }
            });
        let focused_area = tab
            .get_active_pane_id(client_id)
            .and_then(|pane_id| tab.get_pane_with_id(pane_id))
            .map(|pane| {
                let geom = pane.current_geom();
                Viewport {
                    x: geom.x,
                    y: geom.y,
                    rows: geom.rows.as_usize(),
                    cols: geom.cols.as_usize(),
                }
            })
            .unwrap_or(bounds);
        let cursor_position = tab
            .get_active_terminal_cursor_position(client_id)
            .map(|(x, y, _)| Position::new((y + 1) as i32, x as u16));
        let at_cursor = cursor_position
            .map(PopupPlacement::At)
            .unwrap_or(PopupPlacement::CenteredIn(focused_area));
        let popup_placement = match placement {
            PipePopupPlacement::Pane => PopupPlacement::CenteredIn(pane_area.unwrap_or(bounds)),
            PipePopupPlacement::Center => PopupPlacement::CenteredIn(bounds),
            PipePopupPlacement::Mouse => match self.last_mouse_positions.get(&client_id) {
                Some(position) => PopupPlacement::At(*position),
                None => at_cursor,
            },
            PipePopupPlacement::Cursor => at_cursor,
            PipePopupPlacement::TopLeft
            | PipePopupPlacement::TopRight
            | PipePopupPlacement::BottomLeft
            | PipePopupPlacement::BottomRight => match placement.corner() {
                Some(corner) => PopupPlacement::Corner(corner),
                None => PopupPlacement::CenteredIn(bounds),
            },
        };
        let kind = if focused {
            PopupKind::Prompt
        } else {
            PopupKind::Info
        };
        let mut caller_args = BTreeMap::new();
        if let Some(pane_id) = caller_pane_id {
            let pane_id: zellij_utils::data::PaneId = pane_id.into();
            caller_args.insert(
                zellij_utils::prompt::CALLER_PANE_ID_ARG.to_owned(),
                pane_id.to_string(),
            );
        }
        caller_args.insert(
            zellij_utils::prompt::CALLER_PANE_TITLE_ARG.to_owned(),
            caller_title,
        );
        if let Some(caller_tab_name) = caller_tab_name {
            caller_args.insert(
                zellij_utils::prompt::CALLER_TAB_NAME_ARG.to_owned(),
                caller_tab_name,
            );
        }
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::LoadPopup {
                run_plugin_or_alias,
                tab_id,
                tab_index: tab.position,
                client_id,
                placement: popup_placement,
                kind,
                anchor_pane: if focused { pane_in_tab } else { None },
                size: Size {
                    rows: PIPE_POPUP_INITIAL_ROWS,
                    cols: PIPE_POPUP_INITIAL_COLS,
                },
                initial_events: vec![],
                pipe: Some((popup_request, caller_args)),
            });
    }
    pub fn add_popup(
        &mut self,
        plugin_id: u32,
        client_id: ClientId,
        tab_id: usize,
        run_plugin_or_alias: RunPluginOrAlias,
        placement: PopupPlacement,
        kind: PopupKind,
        width: usize,
        height: usize,
        anchor_pane: Option<PaneId>,
    ) -> Result<()> {
        let anchor_still_exists = anchor_pane
            .map(|pane_id| {
                self.tabs
                    .get(&tab_id)
                    .map(|tab| tab.get_pane_with_id(pane_id).is_some())
                    .unwrap_or(false)
            })
            .unwrap_or(true);
        let tab_id = match self.active_tab_ids.get(&client_id) {
            Some(current_tab_id) if kind == PopupKind::Info => *current_tab_id,
            _ => tab_id,
        };
        let client_is_still_on_tab =
            anchor_still_exists && self.active_tab_ids.get(&client_id) == Some(&tab_id);
        if !client_is_still_on_tab {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(plugin_id));
            return Ok(());
        }
        let title = run_plugin_or_alias.location_string();
        let invoked_with = Some(zellij_utils::input::layout::Run::Plugin(
            run_plugin_or_alias,
        ));
        let replaced = match self.tabs.get_mut(&tab_id) {
            Some(tab) => {
                let replaced = tab.open_popup(
                    client_id,
                    plugin_id,
                    placement,
                    kind,
                    width,
                    height,
                    invoked_with,
                    title,
                )?;
                tab.set_popup_anchor(plugin_id, anchor_pane);
                tab.hold_back_popup(plugin_id);
                replaced
            },
            None => return Ok(()),
        };
        for replaced_plugin_id in replaced {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(replaced_plugin_id));
        }
        self.schedule_popup_reveal(plugin_id, POPUP_REVEAL_FALLBACK_MS);
        self.report_popup_state(client_id);
        self.render(None)
    }
    pub fn set_popup_size(&mut self, plugin_id: u32, width: usize, height: usize) -> Result<()> {
        for tab in self.tabs.values_mut() {
            if tab.has_popup_plugin(plugin_id) {
                tab.resize_popup(plugin_id, width, height);
                break;
            }
        }
        self.render(None)
    }
    pub fn reveal_popup(&mut self, plugin_id: u32) -> Result<()> {
        let revealed = self
            .tabs
            .values_mut()
            .any(|tab| tab.reveal_popup(plugin_id));
        if revealed {
            self.render(None)?;
        }
        Ok(())
    }
    fn schedule_popup_reveal(&self, plugin_id: u32, delay_ms: u64) {
        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::RevealPopupAfter {
                plugin_id,
                delay_ms,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::data::{BareKey, ClickedPaneAction, ContextMenuAction, KeyWithModifier};

    #[test]
    fn items_for_the_clicked_pane_or_tab_and_with_explicit_ids_reach_the_plugin() {
        use zellij_utils::data::{ClickedTabAction, Direction};
        let entries = vec![
            ContextMenuEntry::item(
                "Close pane",
                vec![ContextMenuAction::ClickedPane(
                    ClickedPaneAction::CloseFocus,
                )],
            ),
            ContextMenuEntry::item(
                "Move tab left",
                vec![ContextMenuAction::ClickedTab(ClickedTabAction::Move(
                    Direction::Left,
                ))],
            ),
            ContextMenuEntry::Separator,
            ContextMenuEntry::item(
                "Close pane 3",
                vec![Action::CloseFocusByPaneId {
                    pane_id: zellij_utils::data::PaneId::Terminal(3),
                }
                .into()],
            ),
            ContextMenuEntry::item("Detach", vec![Action::Detach.into()]),
        ];
        assert_eq!(entries_available_to_plugins(entries.clone()), entries);
    }

    #[test]
    fn items_that_cannot_reach_the_plugin_are_left_out() {
        use zellij_utils::data::{ClickedTabAction, Direction};
        let entries = vec![
            ContextMenuEntry::item(
                "Move tab up",
                vec![ContextMenuAction::ClickedTab(ClickedTabAction::Move(
                    Direction::Up,
                ))],
            ),
            ContextMenuEntry::item("Detach", vec![Action::Detach.into()]),
        ];
        assert_eq!(
            entries_available_to_plugins(entries),
            vec![ContextMenuEntry::item(
                "Detach",
                vec![Action::Detach.into()]
            )]
        );
    }

    #[test]
    fn the_estimated_menu_width_leaves_room_for_the_shortcuts() {
        let entries = vec![
            ContextMenuEntry::Item {
                label: "Close pane".to_owned(),
                actions: vec![ContextMenuAction::ClickedPane(
                    ClickedPaneAction::CloseFocus,
                )],
            },
            ContextMenuEntry::Separator,
            ContextMenuEntry::Item {
                label: "Detach".to_owned(),
                actions: vec![Action::Detach.into()],
            },
        ];
        let keybinds: KeybindsVec = vec![
            (
                InputMode::Normal,
                vec![(
                    KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier(),
                    vec![Action::SwitchToMode {
                        input_mode: InputMode::Pane,
                    }],
                )],
            ),
            (
                InputMode::Pane,
                vec![(
                    KeyWithModifier::new(BareKey::Char('x')),
                    vec![
                        Action::CloseFocus,
                        Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        },
                    ],
                )],
            ),
        ];
        let (without_keys, height) =
            estimated_context_menu_size(&entries, &vec![], InputMode::Normal);
        let (with_keys, _) = estimated_context_menu_size(&entries, &keybinds, InputMode::Normal);
        assert_eq!(height, 5);
        assert_eq!(without_keys, 17);
        assert_eq!(
            with_keys,
            "Close pane".len() + 4 + "Ctrl p, x".len() + 1 + 3
        );
    }
}
