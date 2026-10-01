use super::Screen;
use crate::panes::PaneId;
use crate::plugins::PluginInstruction;
use crate::tab::{ContextMenuRequest, PopupKind, PopupMouseOutcome, PopupPlacement};
use crate::{ClientId, ServerInstruction};
use std::collections::BTreeMap;
use std::time::Instant;
use zellij_utils::data::{
    ContextMenuContext, ContextMenuEntry, ContextMenuKind, ContextMenuTarget, Event,
    PipePopupPlacement,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::actions::Action;
use zellij_utils::input::context_menu::ContextMenuConfig;
use zellij_utils::input::layout::RunPluginOrAlias;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::pane_size::{Size, Viewport};
use zellij_utils::plugin_api::action::ProtobufAction;
use zellij_utils::position::Position;

pub const CONTEXT_MENU_PLUGIN_ALIAS: &str = "context-menu";
pub const PIPE_POPUP_INITIAL_COLS: usize = 50;
pub const PIPE_POPUP_INITIAL_ROWS: usize = 8;

pub fn estimated_context_menu_size(entries: &[ContextMenuEntry]) -> (usize, usize) {
    let widest_label = entries
        .iter()
        .filter_map(|entry| entry.label())
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0);
    let width = (widest_label + 6).max(16);
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
                .all(|action| ProtobufAction::try_from(action.clone()).is_ok()),
        })
        .collect()
}

impl Screen {
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
    fn client_has_popup(&self, client_id: ClientId) -> bool {
        self.tabs
            .values()
            .any(|tab| tab.has_popup_for_client(client_id))
    }
    fn report_popup_state(&self, client_id: ClientId) {
        let _ = self
            .bus
            .senders
            .send_to_server(ServerInstruction::PopupStateChanged(
                client_id,
                self.client_has_popup(client_id),
            ));
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
                .map(|mut action| {
                    action.fill_context_menu_target(
                        context.target_pane_id(),
                        context.target_tab_id(),
                    );
                    action
                })
                .filter(|action| !action.has_missing_target())
                .collect(),
            _ => vec![],
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
        key_with_modifier: Option<zellij_utils::data::KeyWithModifier>,
    ) -> bool {
        let Some(plugin_id) = self
            .get_active_tab(client_id)
            .ok()
            .and_then(|tab| tab.popup_plugin_id(client_id))
        else {
            return false;
        };
        if let Some(key_with_modifier) = key_with_modifier {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    Some(plugin_id),
                    Some(client_id),
                    Event::Key(key_with_modifier),
                )]));
        }
        true
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
        let (width, height) = estimated_context_menu_size(&entries);
        let anchor = Position::new(context.line as i32, context.column as u16);
        let run_plugin_or_alias =
            match RunPluginOrAlias::from_url(CONTEXT_MENU_PLUGIN_ALIAS, &None, None, None) {
                Ok(run_plugin_or_alias) => run_plugin_or_alias,
                Err(e) => {
                    log::error!("Failed to open context menu: {}", e);
                    return;
                },
            };
        self.request_popup(
            client_id,
            run_plugin_or_alias,
            anchor,
            width,
            height,
            Some(Event::ContextMenu(context.clone(), entries.clone())),
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
    ) {
        let (line, column) =
            self.plugin_relative_to_screen(requesting_plugin_id, client_id, line, column);
        let anchor = Position::new(line as i32, column as u16);
        self.request_popup(client_id, run_plugin_or_alias, anchor, width, height, None);
    }
    fn request_popup(
        &mut self,
        client_id: ClientId,
        run_plugin_or_alias: RunPluginOrAlias,
        anchor: Position,
        width: usize,
        height: usize,
        initial_event: Option<Event>,
    ) {
        self.close_menu_popups_for_client(client_id);
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
                placement: PopupPlacement::At(anchor),
                kind: PopupKind::Menu,
                size: Size {
                    rows: height,
                    cols: width,
                },
                initial_event,
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
        let _ = self
            .bus
            .senders
            .send_to_plugin(PluginInstruction::LoadPopup {
                run_plugin_or_alias,
                tab_id,
                tab_index: tab.position,
                client_id,
                placement: popup_placement,
                kind: PopupKind::Prompt,
                size: Size {
                    rows: PIPE_POPUP_INITIAL_ROWS,
                    cols: PIPE_POPUP_INITIAL_COLS,
                },
                initial_event: None,
                pipe: Some((pipe_id, caller_args)),
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
    ) -> Result<()> {
        let client_is_still_on_tab = self.active_tab_ids.get(&client_id) == Some(&tab_id);
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
            Some(tab) => tab.open_popup(
                client_id,
                plugin_id,
                placement,
                kind,
                width,
                height,
                invoked_with,
                title,
            )?,
            None => return Ok(()),
        };
        for replaced_plugin_id in replaced {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::Unload(replaced_plugin_id));
        }
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
}
